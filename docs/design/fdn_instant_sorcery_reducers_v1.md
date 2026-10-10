# Controller instant/sorcery cost reducers, source preparation

This separate issue110 preparation follows the31-name admission. Neither
Mocking Sprite nor Archmage of Runes is registered or accepted by this work.

The pinned Mage primary at
[a5c90fe1](https://github.com/jackmaiorino/mage/tree/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards)
defines Mocking Sprite as2U2/1 Faerie Rogue with Flying and controller
instant/sorcery spells costing one generic less. Archmage of Runes is3UU3/6
Giant Wizard with the same reduction and an instant/sorcery cast draw-one
trigger. Both constructors were read at that exact commit.

Read-only cost-path consultation identified incomplete shared casting logic:
normal costs and Adventure/Omen already read Standard static modifiers, while
flashback, Escape, Madness and alternative costs bypass them. Flooring a base
cost before X or kicker loses excess reduction; paying additional mana
separately cannot apply one reduction budget to the complete spell cost.

The first source increment adds a combined mana solver accepting raw generic
increases and reductions. It sums base, additional and chosen-X generic cost
in u32 before one floor; colored/hybrid/Phyrexian pip requirements remain in
the existing solver. Existing entrypoints delegate with zero modifiers.
Focused regressions cover kicker, two-X costs, tax/reduction ordering, colored
requirements and totals above255. Read-only review of source8c3b9914 found no
actionable defects. Both Limited-only and combined Standard/Limited test
compilations and all15 mana regressions pass under supported
guardab5c3985b65b40339b8c63a979a7eabd on cores16-17 at BelowNormal priority,
command sequence exit0. Rust1.94.1 and MSVC19.50.35725 are logged. This
qualifies the combined solver increment; casting-path integration is pending.

The next increment collects selected normal/alternative/flashback/Escape/
Madness/Plotted/Adventure/Omen/Bestow mana costs, kicker and Spree surcharge
before adjustment. It retains original nonmana component groups and separates
selected spell-face types from printed creature types. Delve and Convoke stay
explicit and refuse the ordinary mana-only solver until their payment adapters
are wired. Six focused collector regressions use existing registered cards;
native execution and read-only review are pending. The collector is not yet
wired into the engine's offer, pending-decision or payment paths.

Remaining implementation must derive live source modifiers and route every
offer, pending choice, X maximum and final payment through one spell-only
planner. It must include Adventure/Omen types, Plotted taxes, optional costs,
Delve/Convoke and nonmana payment, freezing the determined total before paying
a sacrifice that removes a reducer. Activation/resolution payments stay
outside spell modifiers. The Archmage trigger can reuse CastInstantOrSorcery
and controller draw. Registration, metadata, card gameplay/restore, generated
catalog/profile qualification, CI and integration remain required.
