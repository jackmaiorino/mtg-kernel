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

## Evidence (2026-10-29, games from #415 head 6dfd1f6b1)

1,200 games: 580 and 566 wins, 0 errors, 54 hangs (all with Drake Hatcher), 577 s on 8 cores.
Full-only baseline, without the Drake Hatcher hangs: 0 halts in 293 games. With no halt
anywhere, every Partial card with at least 20 games passes the rule. The admission list does not
change play under `rules-only-all`, so these games stand for the final list.

| Card | Games | Halts | |
| --- | ---: | ---: | --- |
| Curator of Destinies | 232 | 0 | admitted |
| Imprisoned in the Moon | 214 | 0 | admitted |
| Blasphemous Edict | 143 | 0 | admitted |
| Leyline Axe | 108 | 0 | admitted |
| Kykar, Zephyr Awakener | 97 | 0 | admitted |
| Lathril, Blade of the Elves | 88 | 0 | admitted |
| Perforating Artist | 86 | 0 | admitted |
| Liliana, Dreadhorde General | 84 | 0 | admitted |
| Zimone, Paradox Sculptor | 76 | 0 | admitted |
| Consuming Aberration | 63 | 0 | admitted |
| Alesha, Who Laughs at Fate | 60 | 0 | admitted |
| Abyssal Harvester | 55 | 0 | admitted |
| Tinybones, Bauble Burglar | 52 | 0 | admitted |
| Loot, Exuberant Explorer | 48 | 0 | admitted |
| Thousand-Year Storm | 44 | 0 | admitted |
| Muldrotha, the Gravetide | 36 | 0 | admitted |
| Quilled Greatwurm | 32 | 0 | admitted |
| Drakuseth, Maw of Flames | 30 | 0 | admitted |
| Vivien Reid | 30 | 0 | admitted |
| Embercleave | 29 | 0 | admitted |
| Chandra, Flameshaper | 27 | 0 | admitted |
| Sphinx of Forgotten Lore | 27 | 0 | admitted |
| Valkyrie's Call | 26 | 0 | admitted |

With these 23, all 80 sample decks play under `-session rules-only`, against 34 under Prepared.

"Games" counts games where the card was in a deck, not games where it was cast.

The 2026-10-27 run (19 admitted, 70 decks) had 29 errors: attacker and blocker inclusion options
the surface refused (#376), tokens left in a graveyard without a roster entry, and
`InvalidEffectContinuation` halts. None recur. #404 fixed the first, and #415 the halts from
Vampire Gourmand (another trigger of a Gourmand source taken for its own, and its paid draw dropped
by a linked Aura replay) and from Reassembling Skeleton under mixed Crystal prevention.

On 10 deals of `FDN_top_02819_WU` vs `FDN_top_21341_WU` (two decks both sessions can play), 7
finished in both with the same result and decision count, rules-only about 60 times faster. One
finished in both with the same winner but different decision counts (1,329 Prepared, 1,077
rules-only). Its first difference is decision 398, a 10-option effect target menu of the same size
whose first option differs: Prepared normalizes candidate order (`flat_action_v3`), rules-only
keeps the builder's order, so the random pick differs from there on. In one, Prepared refused
(`ravenous_v51:InvalidDecisionRelation`) where rules-only played on, and both outlasted the
timeout in the last (Drake Hatcher, #375).
