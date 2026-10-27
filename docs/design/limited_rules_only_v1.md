# Limited rules-only admission (v1)

Part of #110. The maintainer chose a rules-only tier on 2026-10-26: Partial printings whose rules
#308 already executes can play in self-play, search and DraftZero evaluation before their
consumer stack (owning session, Human, transport, numeric model) exists. `-session rules-only`
plays Full cards plus the printings in `limited_rules_only_v1::RULES_ONLY_ADMITTED_V1`;
`-session rules-only-all` plays every executable printing and is for measurement only. Admission
never changes a printing's Full or Partial status, and no observation leaves the session.

## Rule

`python python/tools/limited_match_v1.py draftzero --pairs 600 --workers 8` plays random vs random
rules-only-all games on both splits of DraftZero's 80 sample decks (seed 1), one process per game
with a 60 s timeout.

- A halted game (engine refusal or halt) or hung game counts against every Partial card in its
  two decks.
- A hung game holding Drake Hatcher counts for no card: that Full card makes every later step
  slower (#375), so those games outlast any timeout whatever else is in them.
- Games with only Full cards halt too, from engine faults no admission can cause. A card is
  admitted when it has at least 20 counted games and its halt rate is not significantly above
  that Full-only rate (one-sided binomial tail at least 0.05).

Halts only catch refusals. Rules correctness rests on #308's own per-card rules tests.

## Evidence (2026-10-27, branch head 04c0b166d plus this list)

1,200 games: 560 and 546 wins, 29 errors, 65 hangs (all with Drake Hatcher), 625 s on 8 cores.
Full-only baseline: 18 halts in 303 games (5.9%).

| Card | Games | Halts | Tail p | |
| --- | ---: | ---: | ---: | --- |
| Curator of Destinies | 226 | 3 | 1.00 | admitted |
| Imprisoned in the Moon | 213 | 3 | 1.00 | admitted |
| Blasphemous Edict | 142 | 4 | 0.97 | admitted |
| Leyline Axe | 107 | 2 | 0.99 | admitted |
| Kykar, Zephyr Awakener | 96 | 1 | 1.00 | admitted |
| Lathril, Blade of the Elves | 87 | 5 | 0.59 | admitted |
| Perforating Artist | 85 | 6 | 0.39 | admitted |
| Liliana, Dreadhorde General | 82 | 5 | 0.54 | admitted |
| Zimone, Paradox Sculptor | 76 | 2 | 0.94 | admitted |
| Consuming Aberration | 63 | 3 | 0.73 | admitted |
| Alesha, Who Laughs at Fate | 59 | 11 | 0.0006 | held |
| Abyssal Harvester | 54 | 3 | 0.63 | admitted |
| Tinybones, Bauble Burglar | 52 | 2 | 0.82 | admitted |
| Loot, Exuberant Explorer | 48 | 0 | 1.00 | admitted |
| Thousand-Year Storm | 43 | 0 | 1.00 | admitted |
| Muldrotha, the Gravetide | 36 | 2 | 0.64 | admitted |
| Quilled Greatwurm | 32 | 1 | 0.86 | admitted |
| Drakuseth, Maw of Flames | 30 | 0 | 1.00 | admitted |
| Vivien Reid | 30 | 1 | 0.84 | admitted |
| Embercleave | 29 | 0 | 1.00 | admitted |
| Chandra, Flameshaper | 27 | 2 | 0.48 | admitted |
| Sphinx of Forgotten Lore | 27 | 0 | 1.00 | admitted |
| Valkyrie's Call | 26 | 1 | 0.80 | admitted |

"Games" counts games where the card was in a deck, not games where it was cast.

Alesha's halts are mostly in games with Flamewake Phoenix's deck, whose forced attack the
inclusion menu gets wrong (#376). Readmit it once #376 is fixed and a rerun clears it.

The errors are engine-wide, not card-specific: attacker or blocker inclusion options the surface
refuses (#376), tokens left in a graveyard without a roster entry (Food, Soldier, Insect, Treasure,
Elf Warrior), and four `InvalidEffectContinuation` halts.

On 10 deals of `FDN_top_02819_WU` vs `FDN_top_21341_WU` (two decks both sessions can play), 7
finished in both with the same result and decision count, rules-only about 60 times faster. One
finished in both with the same winner but different decision counts (1,329 Prepared, 1,077
rules-only). Its first difference is decision 398, a 10-option effect target menu of the same size
whose first option differs: Prepared normalizes candidate order (`flat_action_v3`), rules-only
keeps the builder's order, so the random pick differs from there on. In one, Prepared refused
(`ravenous_v51:InvalidDecisionRelation`) where rules-only played on, and both outlasted the
timeout in the last (Drake Hatcher, #375).
