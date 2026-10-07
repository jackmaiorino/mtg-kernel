# FDN gainlands and life-gain creatures v1

Second milestone-5 batch for issue 110. Every card reuses an engine primitive
that the original fixtures already exercise: dual mana-ability choices, enters
tapped, the gainland enter trigger, the life-gain trigger condition, and the
attack trigger.

| Card | Behavior | Engine recipe |
| --- | --- | --- |
| Bloodfell Caves, Dismal Backwater, Jungle Hollow, Rugged Highlands, Scoured Barrens, Swiftwater Cliffs, Tranquil Cove, Wind-Scarred Crag | Enters tapped; when it enters, gain 1 life; {T}: add either printed color | Same as Blossoming Sands / Thornwood Falls |
| Ajani's Pridemate ({1}{W}, 2/2 Cat Soldier) | Whenever you gain life, put a +1/+1 counter on it | `ControllerGainsLife` bound-source counter, as Exemplar of Light |
| Marauding Blight-Priest ({2}{B}, 3/2 Vampire Cleric) | Whenever you gain life, each opponent loses 1 life | `ControllerGainsLife` + `LoseLife(Opponent, 1)` |
| Sanguine Syphoner ({1}{B}, 1/3 Vampire Warlock) | Whenever it attacks, each opponent loses 1 life and you gain 1 life | `Attacks` + `LoseLife(Opponent, 1)` then `GainLife(Controller, 1)` |

Characteristics come from the XMage card files named in each registry entry.
In this two-player kernel, "each opponent" is the single opponent.

The definitions append as ids 243 through 253. Warlock appends to `Subtype`,
and is added to `CREATURE_TYPES` only under the Limited feature. The feature
catalog moves to `kernel_carddb/v53` (`f0cebc58a1ba113d`) with a new
`FdnGainlandsLifegain` store profile. The keyword-creature profile stays
readable and is refused for mutation. Default builds keep Pauper v34. The
synthetic `FDN_reference_gainlands_lifegain.dck` provides deck coverage.

`mtg-kernel/tests/fdn_gainlands_lifegain_v1.rs` checks ids and
characteristics; that each gainland enters tapped, gains 1 life on resolution
and taps for each of its colors; exact creature costs; one Pridemate counter
per separate life gain; that an opponent's life gain triggers neither
Pridemate nor Blight-Priest; Blight-Priest and Pridemate triggering together;
Syphoner's attack drain feeding both of them (20/20 to 21/18); and Syphoner's
trigger resolving after it leaves the battlefield.
