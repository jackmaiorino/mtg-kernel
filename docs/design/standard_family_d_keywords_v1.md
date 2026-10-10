# MageZero Standard family D: new set keywords v1

Third batch of the MageZero Standard catalog (`kernel_carddb_standard/v4`,
`0x58c7c4e68b6ef277`), covering mechanic family D of
`docs/reports/standard_magezero_inventory_v1.md`.
Every card's printed behavior was read from its XMage card file (path in the
registry entry). All new behavior is gated on the `standard-magezero-fixtures`
feature, so the Pauper and FDN Limited identities are unchanged.

## Cards

| Card | Deck use | Mechanics |
| --- | --- | --- |
| Emberheart Challenger | Mono-R | Prowess, valiant |
| Burnout Bashtronaut | Mono-R | Start your engines!, max speed, menace |
| Nova Hellkite | Mono-R | Warp, flying, haste, enters ping |
| Full Bore | Mono-R | Warp-conditional pump |
| Iridescent Vinelasher | Mono-B | Offspring, landfall |
| Aloe Alchemist | Mono-G | Plot, becomes-plotted trigger |
| Forsaken Miner | Mono-B | Crimes, returns from graveyard, can't block |
| Axebane Ferox | Mono-G | Ward—collect evidence 4, deathtouch, haste |
| Hopeful Initiate | Mono-W | Training, counter-removal activation |
| Chrome Host Seedshark | Mono-U | Incubate, Incubator token transform |
| Brutal Cathar / Moonrage Brute | Mono-W | Daybound, nightbound, ward—pay 3 life |
| Knight-Errant of Eos | Mono-W | Convoke, look-at-top choice |
| Flourishing Bloom-Kin | Mono-G | +1/+1 per Forest (disguise omitted, below) |
| Monastery Swiftspear, Heartfire Hero, Slickshot Show-Off | two-color | Prowess, valiant, plot |
| Sanguine Evangelist, Darkstar Augur | two-color | Battle cry, offspring, upkeep reveal |
| Ruin-Lurker Bat | two-color | Descend end-step scry |
| Pawpatch Recruit, Manifold Mouse | two-color | Offspring, opponent-targeting trigger, combat keyword choice |
| Yotian Frontliner | two-color | Unearth |
| Cori-Steel Cutter | two-color | Flurry, Monk token attach |
| Graveyard Trespasser / Graveyard Glutton | two-color | Daybound, ward—discard a card, back-face triggers |
| Overlord of the Mistmoors | two-color | Impending |
| Enduring Curiosity, Enduring Innocence | two-color | Enduring return as an enchantment |
| Make Disappear | two-color | Casualty 1, counter unless pays {2} |
| Phantom Interference | two-color | Spree |

Tokens: Iridescent Vinelasher, Darkstar Augur, Pawpatch Recruit and Manifold
Mouse offspring tokens, Incubator, Bat, Monk, White Insect and Spirit.

## Engine additions

- **Speed** is per-player state. Start your engines! sets it to 1, it rises
  once per turn when an opponent loses life on your turn, and max speed
  abilities read 4.
- **Warp** is an alternative cost castable only from hand. The permanent
  remembers it was warped; its end-step trigger exiles it and grants a
  later-turn exile permission to recast it for its normal cost.
- **Offspring** reuses kicker's optional additional cost. A paid cast's enters
  trigger creates a separate 1/1 token definition with the same name, types
  and abilities.
- **Crimes** are logged when a spell, activation or trigger finishes targeting
  an opponent, an opponent's permanent or spell, or a card in an opponent's
  graveyard. Copies never commit crimes.
- **Ward** gains collect evidence, pay life (back face only) and discard a
  card, alongside generic ward.
- **Day and night** starts when a daybound permanent appears and changes as
  each turn begins. Daybound permanents follow it at the state check and
  enter transformed at night. `TriggeredAbilityDef::face_index` lets a
  transform back face carry its own triggers.
- **Convoke** is the card's alternative cost, so the existing cast-mode choice
  decides whether to convoke. The number of creatures tapped rides onto the
  permanent.
- **Unearth** is a graveyard activation. The returned creature gains haste,
  its end-step trigger exiles it, and a leave-the-battlefield replacement
  exiles it instead of any other zone (including state-based deaths).
- **Flurry** triggers on the controller's second spell each turn. **Descend**
  is tracked per turn for Ruin-Lurker Bat.
- **Impending** is an alternative cost from hand. The permanent enters with
  time counters, is not a creature while it has any, and loses one at each of
  its controller's end steps.
- **Enduring** creatures that die return at once as noncreature enchantments.
- **Casualty** shares Bargain's sacrifice-one optional cost path. A casualty
  cast puts a copy with the same targets on the stack.
- **Spree** is modeled as one printed mode per mode set (Phantom Interference:
  Spirit, counter, both). Each mode adds its `+{N}` surcharge to the cast cost,
  and only affordable mode sets are offered.
- Triggered abilities with "up to" targets can now finish early, and the
  new target specs are another creature you control, a creature you control
  with a subtype, and up to one card in a graveyard.
- **Rules vector.** Family D's trigger conditions, costs, Ward costs and
  effect ops have meaning rows (`rules_vector_v1/meaning/effect_h.rs` for
  the ops). Its name-keyed statics and Spree surcharges are read through
  `standard_keywords_v1::rules_vector_statics` and `spree_extra_generic`.

## Simplifications

These keep the decision surface unchanged. Each is a deliberate departure
from the printed rules.

- **Ward payments are automatic.** The targeting player always pays a ward
  cost they can pay: collect evidence exiles the smallest-total qualifying
  subset, pay life pays when it leaves them above 0, and ward—discard
  discards their lowest-mana-value card.
- **Convoke and training costs are chosen deterministically.** Convoke taps
  creatures in a fixed order, and Hopeful Initiate's counter removal takes
  from the creature with the most +1/+1 counters.
- **Casualty's copy keeps the original's targets** and is created immediately
  as part of casting, not by a separate trigger. No new targets are offered.
- **Cori-Steel Cutter's attach choice** is made when the trigger resolves,
  before the Monk token exists.
- **Overlord of the Mistmoors' end-step trigger** is only collected while it
  has time counters.
- **Not-a-creature permanents** (impending, Enduring enchantments) are handled
  by the type query, attack and block checks. Other code that reads printed
  types directly still sees a creature.
- **Unearth's haste** lasts until end of turn rather than indefinitely, which
  is equivalent since the creature is exiled at end of turn.
- **Phantom Interference's both-modes set** creates its Spirit just before the
  counter program runs, in printed order, because a counter-unless-pays
  program must stay rooted.
- **Flourishing Bloom-Kin has no disguise.** The engine has no face-down
  objects, so it can only be hard-cast and its turned-face-up trigger never
  fires. Because a printed ability is missing, its registry entry is
  `partial` and full-deck admission refuses it.

## Deferred

- **Monstrous Rage** needs the creature-Aura static profile from the FDN
  token and Aura batch (#168). That batch has merged, so it is the next
  follow-up.
- **Zoetic Glyph** needs an artifact Aura that makes its host a creature.
  Family E's vehicles need the same artifact-becomes-creature primitive, so
  it waits for that batch rather than building it twice. Discover lands with
  it.
- **Collector's Cage** needs hideaway's face-down exile.

## Tests

`mtg-kernel/tests/standard_family_d_v1.rs` covers each card's printed
behavior, the keyword interactions above and every simplification.
