# MageZero Standard family D: new set keywords v1

Family D of the MageZero Standard v7 completion candidate covers the set keywords
used by the unchanged deck pool. Printed behavior follows each card's XMage source
(path in its registry entry) and official rules where needed. The completion includes
all family D cards and Full admission flags; combined native/runtime verification is
pending in `docs/reports/standard_completion_v1.md`. The original v4 batch identities
are historical, and the v7 hash must be frozen from the integration build. Pauper and
FDN registry files and catalog identities remain separate.

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
| Flourishing Bloom-Kin | Mono-G | +1/+1 per Forest, disguise and face-up Forest choices |
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
- **Convoke** asks for the creature subset and each tapped creature's mana
  contribution while paying the spell. The number tapped rides onto the
  permanent for Knight-Errant's subsequent card selection.
- **Unearth** is a graveyard activation. The returned creature gains haste,
  its end-step trigger exiles it, and a leave-the-battlefield replacement
  exiles it instead of any other zone (including state-based deaths).
- **Flurry** triggers on the controller's second spell each turn. **Descend**
  is tracked per turn for Ruin-Lurker Bat.
- **Impending** is an alternative cost from hand. The permanent enters with
  time counters, is not a creature while it has any, and loses one at each of
  its controller's end steps.
- **Enduring** creatures have a death trigger that returns their exact
  graveyard incarnation as a noncreature enchantment under its owner's control.
- **Casualty** shares Bargain's sacrifice-one optional cost path. Its copy
  trigger creates a spell copy with independently chosen legal targets.
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

## Completion implementation

The completion branch replaces the earlier deterministic approximations with
player choices: optional Ward payments and evidence/discard subsets, convoke
subsets and Knight-Errant's zero-through-two selection, Hopeful Initiate counter
payments, and a real casualty copy trigger with independent target choices.
Cathar returns every card linked to the departing incarnation immediately;
speed increases through an ordinary triggered ability. Effective creature
eligibility is used for Enduring and impending sacrifice costs.

Monstrous Rage creates a Monster Role attached to its exact creature target.
The Role rule keeps only the newest Role controlled by a player on that host.
Zoetic Glyph uses the shared timestamped artifact animation profile and its
death trigger discovers 3. Discover offers a real free cast, with the rejected
exiles returned in random bottom order after casting completes.

Flourishing Bloom-Kin can be cast face down for three generic mana. Its
face-down spell and permanent have the public 2/2 colorless creature profile
and ward 2. The controller pays 4G to turn it face up as a special action,
then its trigger offers zero through two Forests, choosing the first for the
battlefield tapped and the second for hand. Index 255 in the existing
ActivateAbility action envelope is reserved for this special action, with
a distinct public TurnFaceUp semantic and human label; it
creates no activated ability, permits ordinary priority timing, and retains
priority. Losing the face-up disguise ability prevents paying that cost.

Collector's Cage privately chooses one of the top five cards, exiles it face
down, and puts the rest on the bottom in random order. Its exact source link
survives the source leaving. Each player who controlled that incarnation can
look at the hidden card. Its activation adds the counter before checking for
three distinct controlled creature powers and offering the linked card as a
free spell or a legal land play.

Generic public references mask face-down definitions. Modern observations
carry actual identities in a separate authorized-viewer knowledge list; flat
model rows and action references use the same actor-authorized identity.
Existing observations and hashes omit these fields when no face-down cards
exist. Free casting uses the shared parent-resolution continuation and the
ordinary target, mode, additional-cost, and tax pipeline.

Full flags enable completion-candidate admission. Compilation and the combined
behavior tests remain pending; source and inventory checks do not replace them.

## Tests

`mtg-kernel/tests/standard_family_d_v1.rs` covers implemented behavior,
candidate admission, all three Spree mode payments, two pending Incubator
activations, battle cry across zone changes, Seedshark with an Omen spell,
and a stolen Enduring creature's death trigger and owner return. Casualty's
transformed-Incubator regression exercises effective creature eligibility
and paid provenance after the sacrifice resets its face. The targeted
Standard event library test checks suppression of Heartfire's death ability
from the pre-departure ability snapshot. Hosted CI runs these groups on
Linux and Windows. Integration requires passing checks at the reviewed
head; earlier-head CI alone does not qualify a changed implementation.
